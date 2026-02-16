import { Component, Input, ChangeDetectionStrategy } from '@angular/core';
import { CommonModule } from '@angular/common';
import { cn } from '../utils';

@Component({
  selector: 'app-table',
  standalone: true,
  imports: [CommonModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="relative w-full overflow-auto">
      <table [class]="cn('w-full caption-bottom text-sm', className)">
        <ng-content></ng-content>
      </table>
    </div>
  `,
})
export class TableComponent {
  @Input() className = '';
  cn = cn;
}

@Component({
  selector: 'app-table-header',
  standalone: true,
  imports: [CommonModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <thead [class]="cn('[&_tr]:border-b', className)">
      <ng-content></ng-content>
    </thead>
  `,
})
export class TableHeaderComponent {
  @Input() className = '';
  cn = cn;
}

@Component({
  selector: 'app-table-body',
  standalone: true,
  imports: [CommonModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <tbody [class]="cn('[&_tr:last-child]:border-0', className)">
      <ng-content></ng-content>
    </tbody>
  `,
})
export class TableBodyComponent {
  @Input() className = '';
  cn = cn;
}

@Component({
  selector: 'app-table-row',
  standalone: true,
  imports: [CommonModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <tr [class]="cn('border-b transition-colors hover:bg-muted/50 data-[state=selected]:bg-muted', className)">
      <ng-content></ng-content>
    </tr>
  `,
})
export class TableRowComponent {
  @Input() className = '';
  cn = cn;
}

@Component({
  selector: 'app-table-head',
  standalone: true,
  imports: [CommonModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <th [class]="cn('h-12 px-4 text-left align-middle font-medium text-muted-foreground [&:has([role=checkbox])]:pr-0', className)">
      <ng-content></ng-content>
    </th>
  `,
})
export class TableHeadComponent {
  @Input() className = '';
  cn = cn;
}

@Component({
  selector: 'app-table-cell',
  standalone: true,
  imports: [CommonModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <td [class]="cn('p-4 align-middle [&:has([role=checkbox])]:pr-0', className)">
      <ng-content></ng-content>
    </td>
  `,
})
export class TableCellComponent {
  @Input() className = '';
  cn = cn;
}